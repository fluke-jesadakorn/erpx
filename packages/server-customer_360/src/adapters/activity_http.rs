//! Native Executive customer-activity HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, one bounded
//! cross-customer activity search or record read. Current Account authority,
//! selected Company/branch/department scope, Support history purchase and the
//! serializable owner unit remain composition responsibilities behind the
//! backend trait.
use crate::domain::timeline::{
    CustomerTimelineCursor, ExecutiveCustomerActivityQuery, ExecutiveCustomerActivityRow, MAX_LIMIT,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const QUERY_LIMIT: usize = 8 * 1024;

pub type CustomerActivityHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct CustomerActivityHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub query: ExecutiveCustomerActivityQuery,
}

pub trait CustomerActivityHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: CustomerActivityHttpReadRequest<'a>,
    ) -> CustomerActivityHttpFuture<'a, Vec<ExecutiveCustomerActivityRow>>;
}

pub struct CustomerActivityHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn CustomerActivityHttpBackend>,
}

struct CustomerActivityHttpState {
    targets: BTreeMap<(String, String), CustomerActivityHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_ACTIVITY_REQUEST_INVALID",
        "Use one bounded customer activity request.",
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
        "CUSTOMER_ACTIVITY_CONFIGURATION_INVALID",
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
    state: &'a CustomerActivityHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a CustomerActivityHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}
fn require_origin(
    target: &CustomerActivityHttpTarget,
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

fn parse(query: &str) -> Result<ExecutiveCustomerActivityQuery, BusinessError> {
    if query.len() > QUERY_LIMIT {
        return Err(invalid());
    }
    let mut from = None;
    let mut to = None;
    let mut search = None;
    let mut cursor_at = None;
    let mut cursor_id = None;
    let mut limit = None;
    let mut record_id = None;
    for pair in query.split('&').filter(|value| !value.is_empty()) {
        let (name, value) = pair.split_once('=').ok_or_else(invalid)?;
        let value = percent_decode(value)?;
        match name {
            "from" if from.is_none() => from = Some(integer(&value)?),
            "to" if to.is_none() => to = Some(integer(&value)?),
            "query" if search.is_none() => search = Some(value),
            "cursorAt" if cursor_at.is_none() => cursor_at = Some(integer(&value)?),
            "cursorId" if cursor_id.is_none() => cursor_id = Some(value),
            "limit" if limit.is_none() => limit = Some(integer(&value)?),
            "recordId" if record_id.is_none() => record_id = Some(value),
            _ => return Err(invalid()),
        }
    }
    let from = from.ok_or_else(invalid)?;
    let to = to.ok_or_else(invalid)?;
    let limit = match limit.unwrap_or(30) {
        value if (1..=MAX_LIMIT as i64).contains(&value) => value as usize,
        _ => return Err(invalid()),
    };
    let cursor = match (cursor_at, cursor_id) {
        (None, None) => None,
        (Some(at), Some(id)) => Some(CustomerTimelineCursor { at, id }),
        _ => return Err(invalid()),
    };
    Ok(ExecutiveCustomerActivityQuery {
        from,
        to,
        query: search,
        cursor,
        limit,
        record_id,
    })
}

async fn read(
    State(state): State<Arc<CustomerActivityHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().unwrap_or("").to_owned();
    let result: Result<Vec<ExecutiveCustomerActivityRow>, BusinessError> = async move {
        let target = target(&state, &headers, &slug)?;
        require_origin(target, &headers)?;
        let query = parse(&query)?;
        query.validate()?;
        target
            .backend
            .read(CustomerActivityHttpReadRequest {
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

pub fn customer_activity_http_router(
    targets: Vec<CustomerActivityHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/customers/activity",
            get(read),
        )
        .with_state(Arc::new(CustomerActivityHttpState { targets: map })))
}
