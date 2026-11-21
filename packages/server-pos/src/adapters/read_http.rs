//! Owner-local HTTP ingress for the native POS workspace/day read.
//!
//! Transport facts only: registered Host/Holding target, exact origin, exact
//! bounded query parameters and business-error mapping. Normal Account session
//! resolution, the selected Company/branch, package admission and the
//! serializable owner read remain composition responsibilities behind
//! [`PosWorkspaceHttpBackend`]. This router never mints a session, actor or
//! entitlement and never runs SQL.
use crate::domain::read::{
    MAX_QUERY_BYTES, MAX_SAFE_INTEGER, MAX_SALES, POS_PERMISSION_NAMES, PosWorkspaceQuery,
    PosWorkspaceTab, SEARCH_MAX_CHARS, WORKSPACE_KEYS, request_invalid,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type PosWorkspaceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct PosWorkspaceHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub query: PosWorkspaceQuery,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct PosWorkspaceHttpResponse {
    /// Owner workspace payload, exactly [`WORKSPACE_KEYS`].
    pub data: serde_json::Value,
    /// Validated current context rendered by the composition backend.
    pub context: serde_json::Value,
}

impl PosWorkspaceHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({ "data": self.data, "context": self.context })
    }
}

/// Composition-owned Account session, scope, package admission and owner-unit
/// adapter. The router never derives a permission, Company, branch or purchase.
pub trait PosWorkspaceHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: PosWorkspaceHttpRequest<'a>,
    ) -> PosWorkspaceHttpFuture<'a, PosWorkspaceHttpResponse>;
}

pub struct PosWorkspaceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PosWorkspaceHttpBackend>,
}

pub struct PosWorkspaceHttpState {
    targets: BTreeMap<(String, String), PosWorkspaceHttpTarget>,
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

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_SOURCE_INVALID",
        "The POS workspace could not be read back.",
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

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

/// Percent-decode one query component. `+` is a space, as in the retained
/// `URLSearchParams`/form encoding.
fn decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' => {
                let high = bytes.get(index + 1).copied().ok_or_else(request_invalid)?;
                let low = bytes.get(index + 2).copied().ok_or_else(request_invalid)?;
                let hex = |byte: u8| -> Option<u8> {
                    match byte {
                        b'0'..=b'9' => Some(byte - b'0'),
                        b'a'..=b'f' => Some(byte - b'a' + 10),
                        b'A'..=b'F' => Some(byte - b'A' + 10),
                        _ => None,
                    }
                };
                decoded.push(
                    hex(high).ok_or_else(request_invalid)? * 16
                        + hex(low).ok_or_else(request_invalid)?,
                );
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|_| request_invalid())
}

/// Exact retained query shape: `sale`, `q`, `tab` and `limit`, each at most
/// once, with the retained bounds. `cursor`/`finance` history pagination stays
/// on the retained work-projection read and is rejected here rather than
/// silently ignored.
fn parse_query(query: Option<&str>) -> Result<PosWorkspaceQuery, BusinessError> {
    let mut parsed = PosWorkspaceQuery::default();
    let Some(query) = query.filter(|value| !value.is_empty()) else {
        return Ok(parsed);
    };
    if query.len() > MAX_QUERY_BYTES {
        return Err(request_invalid());
    }
    let mut seen = [false; 4];
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').ok_or_else(request_invalid)?;
        let name = decode(name)?;
        let value = decode(value)?;
        let slot = match name.as_str() {
            "sale" => 0,
            "q" => 1,
            "tab" => 2,
            "limit" => 3,
            _ => return Err(request_invalid()),
        };
        if seen[slot] {
            return Err(request_invalid());
        }
        seen[slot] = true;
        match name.as_str() {
            "sale" if !value.is_empty() => parsed.sale_id = Some(value),
            "q" => parsed.search = value,
            "tab" if !value.is_empty() => {
                parsed.tab = Some(PosWorkspaceTab::parse(&value).ok_or_else(request_invalid)?);
            }
            "limit" => {
                if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(request_invalid());
                }
                let limit: usize = value.parse().map_err(|_| request_invalid())?;
                if !(1..=MAX_SALES).contains(&limit) {
                    return Err(request_invalid());
                }
                parsed.limit = limit;
            }
            _ => {}
        }
    }
    if parsed.search.chars().count() > SEARCH_MAX_CHARS
        || parsed.search.chars().any(char::is_control)
    {
        return Err(request_invalid());
    }
    parsed.validate()?;
    Ok(parsed)
}

fn integer(value: &serde_json::Value) -> bool {
    value
        .as_i64()
        .is_some_and(|number| (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&number))
}

/// Exact bounded backend response: the workspace key set, array/object kinds,
/// the fixed permission map and the fixed summary/entitlement shapes.
fn valid_data(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    if object.len() != WORKSPACE_KEYS.len()
        || !WORKSPACE_KEYS.iter().all(|key| object.contains_key(*key))
    {
        return false;
    }
    if !object["registers"].is_array()
        || !object["shifts"].is_array()
        || !object["cashMovements"].is_array()
        || !object["sales"].is_array()
        || !object["lines"].is_array()
        || !object["payments"].is_array()
        || !(object["settings"].is_null() || object["settings"].is_object())
        || !(object["sale"].is_null() || object["sale"].is_object())
        || !object["cashSummary"].is_object()
        || !object["day"].is_object()
        || !object["permissions"].is_object()
        || !object["entitlements"].is_object()
        || !(object["purchases"].is_null() || object["purchases"].is_object())
        || !object["businessDate"].is_string()
        || !object["userId"].is_string()
        || !object["companyId"].is_string()
        || !(object["branchId"].is_null() || object["branchId"].is_string())
    {
        return false;
    }
    let permissions = &object["permissions"];
    if permissions.as_object().is_none_or(|map| {
        map.len() != POS_PERMISSION_NAMES.len()
            || !POS_PERMISSION_NAMES
                .iter()
                .all(|name| map.get(*name).is_some_and(serde_json::Value::is_boolean))
    }) {
        return false;
    }
    let summary = &object["cashSummary"];
    if summary.as_object().is_none_or(|map| {
        map.len() != 4
            || [
                "pendingCount",
                "pendingMinor",
                "approvedCount",
                "approvedMinor",
            ]
            .iter()
            .any(|key| map.get(*key).is_none_or(|value| !integer(value)))
    }) {
        return false;
    }
    let entitlements = &object["entitlements"];
    if entitlements.as_object().is_none_or(|map| {
        map.len() != 2
            || !map
                .get("packageEnforced")
                .is_some_and(serde_json::Value::is_boolean)
            || !map
                .get("roomChargeEnabled")
                .is_some_and(serde_json::Value::is_boolean)
    }) {
        return false;
    }
    let day = &object["day"];
    day.as_object().is_some_and(|map| {
        map.len() == 3
            && map
                .get("businessDate")
                .is_some_and(serde_json::Value::is_string)
            && map
                .get("state")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|state| matches!(state, "open" | "prepared" | "confirmed"))
            && map
                .get("close")
                .is_some_and(|close| close.is_null() || close.is_object())
    })
}

impl PosWorkspaceHttpState {
    pub fn new(targets: Vec<PosWorkspaceHttpTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || !valid_slug(&target.holding_slug)
                || !valid_origin(&target.origin)
                || configured
                    .insert((target.host.clone(), target.holding_slug.clone()), target)
                    .is_some()
            {
                return Err(BusinessError::invalid(
                    "POS_WORKSPACE_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        Ok(Self {
            targets: configured,
        })
    }

    /// Exact GET handler used by both the Router and owner tests.
    pub async fn read(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Response {
        match self.read_inner(headers, holding_slug, query).await {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn read_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Result<serde_json::Value, BusinessError> {
        let host = unique_header(headers, header::HOST.as_str())?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        if unique_header(headers, header::ORIGIN.as_str())?
            .is_some_and(|value| value != target.origin)
        {
            return Err(origin_forbidden());
        }
        let query = parse_query(query)?;
        let response = target
            .backend
            .read(PosWorkspaceHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                query,
                headers,
            })
            .await?;
        if !valid_data(&response.data) || !response.context.is_object() {
            return Err(source_invalid());
        }
        Ok(response.value())
    }
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

fn bounded(value: serde_json::Value) -> Response {
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

pub fn pos_workspace_router(targets: Vec<PosWorkspaceHttpTarget>) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pos/workspace",
            get(read_workspace),
        )
        .with_state(Arc::new(PosWorkspaceHttpState::new(targets)?)))
}

async fn read_workspace(
    State(state): State<Arc<PosWorkspaceHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    state
        .read(request.headers(), &slug, request.uri().query())
        .await
}
