//! Native Executive customer-activity metric HTTP source.
//!
//! Transport facts only: a registered Host/Holding target, one bounded
//! `from`/`to`/`breakdown` query and the counted groups. Current Account
//! authority, selected Company/branch/department scope, Support history
//! purchase and the serializable owner unit remain composition responsibilities
//! behind [`CustomerActivityMetricHttpBackend`].
use crate::domain::executive_timeline::{
    CustomerActivityBreakdown, CustomerActivityMetricGroup, CustomerActivityMetricQuery,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const QUERY_LIMIT: usize = 1024;

pub type CustomerActivityMetricHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct CustomerActivityMetricHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub query: CustomerActivityMetricQuery,
}

pub trait CustomerActivityMetricHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: CustomerActivityMetricHttpReadRequest<'a>,
    ) -> CustomerActivityMetricHttpFuture<'a, Vec<CustomerActivityMetricGroup>>;
}

pub struct CustomerActivityMetricHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn CustomerActivityMetricHttpBackend>,
}

struct CustomerActivityMetricHttpState {
    targets: BTreeMap<(String, String), CustomerActivityMetricHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_ACTIVITY_METRIC_REQUEST_INVALID",
        "Use one bounded customer activity metric request.",
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
        "CUSTOMER_ACTIVITY_METRIC_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
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
fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
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
    state: &'a CustomerActivityMetricHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a CustomerActivityMetricHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn require_origin(
    target: &CustomerActivityMetricHttpTarget,
    headers: &HeaderMap,
) -> Result<(), BusinessError> {
    let origin = unique_header(headers, header::ORIGIN)?;
    if origin.is_some_and(|value| value != target.origin) {
        return Err(origin_forbidden());
    }
    Ok(())
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
                return Err(invalid());
            };
            out.push(high * 16 + low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| invalid())
}
fn integer(value: &str) -> Result<i64, BusinessError> {
    let parsed = value.parse::<i64>().map_err(|_| invalid())?;
    if parsed.to_string() != value {
        return Err(invalid());
    }
    Ok(parsed)
}

fn parse(query: &str) -> Result<CustomerActivityMetricQuery, BusinessError> {
    if query.len() > QUERY_LIMIT {
        return Err(invalid());
    }
    let mut from = None;
    let mut to = None;
    let mut breakdown = None;
    for pair in query.split('&').filter(|value| !value.is_empty()) {
        let (name, value) = pair.split_once('=').ok_or_else(invalid)?;
        let value = percent_decode(value)?;
        match name {
            "from" if from.is_none() => from = Some(integer(&value)?),
            "to" if to.is_none() => to = Some(integer(&value)?),
            "breakdown" if breakdown.is_none() => {
                breakdown = Some(match value.as_str() {
                    "month" => CustomerActivityBreakdown::Month,
                    "source_type" => CustomerActivityBreakdown::SourceType,
                    _ => return Err(invalid()),
                });
            }
            _ => return Err(invalid()),
        }
    }
    let query = CustomerActivityMetricQuery {
        from: from.ok_or_else(invalid)?,
        to: to.ok_or_else(invalid)?,
        breakdown: breakdown.ok_or_else(invalid)?,
    };
    query.validate()?;
    Ok(query)
}

async fn read(
    State(state): State<Arc<CustomerActivityMetricHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().unwrap_or("").to_owned();
    let result = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers)?;
        let query = parse(&query)?;
        target
            .backend
            .read(CustomerActivityMetricHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                query,
            })
            .await
    }
    .await;
    match result {
        Ok(groups) => bounded(serde_json::json!({"data":{"groups": groups}})),
        Err(error) => error_response(error),
    }
}

pub fn customer_activity_metric_http_router(
    targets: Vec<CustomerActivityMetricHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/customers/activity-metric",
            get(read),
        )
        .with_state(Arc::new(CustomerActivityMetricHttpState {
            targets: configured,
        })))
}
