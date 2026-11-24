//! Native Customer 360 timeline activity HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target and one bounded
//! federated activity read. Current Account authority, selected
//! Company/branch/department scope, Support history purchase and the
//! serializable owner unit remain composition responsibilities behind the
//! backend trait.
use crate::domain::timeline_activity::{
    CustomerTimelineActivityQuery, CustomerTimelineActivityRow, MAX_QUERY_LEN,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const QUERY_LIMIT: usize = 2 * 1024;

pub type CustomerTimelineActivityHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct CustomerTimelineActivityHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub query: CustomerTimelineActivityQuery,
}

pub trait CustomerTimelineActivityHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: CustomerTimelineActivityHttpReadRequest<'a>,
    ) -> CustomerTimelineActivityHttpFuture<'a, Vec<CustomerTimelineActivityRow>>;
}

pub struct CustomerTimelineActivityHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn CustomerTimelineActivityHttpBackend>,
}

struct CustomerTimelineActivityHttpState {
    targets: BTreeMap<(String, String), CustomerTimelineActivityHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_TIMELINE_ACTIVITY_REQUEST_INVALID",
        "Use one bounded customer timeline activity request.",
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
        "CUSTOMER_TIMELINE_ACTIVITY_CONFIGURATION_INVALID",
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
    state: &'a CustomerTimelineActivityHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a CustomerTimelineActivityHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn require_origin(
    target: &CustomerTimelineActivityHttpTarget,
    headers: &HeaderMap,
) -> Result<(), BusinessError> {
    let origin = unique_header(headers, header::ORIGIN)?;
    if origin.is_some_and(|value| value != target.origin) {
        return Err(origin_forbidden());
    }
    Ok(())
}
fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(invalid());
            }
            let high = hex(bytes[index + 1]).ok_or_else(invalid)?;
            let low = hex(bytes[index + 2]).ok_or_else(invalid)?;
            out.push(high * 16 + low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| invalid())
}
fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
fn integer(value: &str) -> Result<i64, BusinessError> {
    let parsed = value.parse::<i64>().map_err(|_| invalid())?;
    if parsed.to_string() != value {
        return Err(invalid());
    }
    Ok(parsed)
}

fn parse(query: &str) -> Result<CustomerTimelineActivityQuery, BusinessError> {
    if query.len() > QUERY_LIMIT {
        return Err(invalid());
    }
    let mut from = None;
    let mut to = None;
    let mut customer_id = None;
    let mut search = None;
    for pair in query.split('&').filter(|value| !value.is_empty()) {
        let (name, value) = pair.split_once('=').ok_or_else(invalid)?;
        let value = percent_decode(value)?;
        match name {
            "from" if from.is_none() => from = Some(integer(&value)?),
            "to" if to.is_none() => to = Some(integer(&value)?),
            "customerId" if customer_id.is_none() => customer_id = Some(value),
            "query" if search.is_none() => search = Some(value),
            _ => return Err(invalid()),
        }
    }
    let from = from.ok_or_else(invalid)?;
    let to = to.ok_or_else(invalid)?;
    if search.as_deref().is_some_and(|value| value.len() > MAX_QUERY_LEN) {
        return Err(invalid());
    }
    let query = CustomerTimelineActivityQuery {
        from,
        to,
        customer_id,
        query: search,
    };
    query.validate()?;
    Ok(query)
}

async fn read(
    State(state): State<Arc<CustomerTimelineActivityHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().unwrap_or("").to_owned();
    let result: Result<Vec<CustomerTimelineActivityRow>, BusinessError> = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers)?;
        let query = parse(&query)?;
        target
            .backend
            .read(CustomerTimelineActivityHttpReadRequest {
                host: unique_header(&headers, header::HOST)?.ok_or_else(missing)?,
                holding_slug: &slug,
                headers: &headers,
                query,
            })
            .await
    }
    .await;
    match result {
        Ok(rows) => bounded(serde_json::json!({"data":{"rows": rows}})),
        Err(error) => error_response(error),
    }
}

pub fn customer_timeline_activity_http_router(
    targets: Vec<CustomerTimelineActivityHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut map = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || map
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/customers/timeline-activity",
            get(read),
        )
        .with_state(Arc::new(CustomerTimelineActivityHttpState { targets: map })))
}
