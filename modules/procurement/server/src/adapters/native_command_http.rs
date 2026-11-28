//! Native axum command ingress for the Procurement owner-write slice.
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 256 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeCommand {
    pub kind: String,
    pub values: Vec<Value>,
    #[serde(default)]
    pub now: i64,
}

pub type NativeCommandHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, BusinessError>> + Send + 'a>>;
pub struct NativeCommandHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: &'a NativeCommand,
    pub key: &'a str,
}
pub trait NativeCommandHttpBackend: Send + Sync {
    fn execute<'a>(&'a self, request: NativeCommandHttpRequest<'a>) -> NativeCommandHttpFuture<'a>;
}
pub struct NativeCommandHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn NativeCommandHttpBackend>,
}
struct AppState {
    targets: BTreeMap<(String, String), NativeCommandHttpTarget>,
    companies: BTreeMap<String, Arc<dyn NativeCommandHttpBackend>>,
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PROCUREMENT_NATIVE_COMMAND_INVALID",
        "Use one bounded native Procurement command.",
    )
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn status(k: FailureKind) -> StatusCode {
    match k {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}
fn error_response(e: BusinessError) -> Response {
    (
        status(e.kind),
        Json(serde_json::json!({"error":{"code":e.code,"message":e.message}})),
    )
        .into_response()
}
fn bounded(body: Value) -> Response {
    let mut r = (StatusCode::OK, Json(body)).into_response();
    for (n, v) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        r.headers_mut().insert(n, HeaderValue::from_static(v));
    }
    r
}
fn unique(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut v = headers.get_all(name).iter();
    let first = v.next();
    if v.next().is_some() {
        return Err(invalid());
    }
    first.map(|x| x.to_str().map_err(|_| invalid())).transpose()
}
fn valid_origin(s: &str) -> bool {
    s.strip_prefix("https://").is_some_and(|rest| {
        let (h, _) = match rest.rsplit_once(':') {
            Some((h, _)) => (h, Some(())),
            None => (rest, None),
        };
        !h.is_empty()
            && !h
                .bytes()
                .any(|b| matches!(b, b'/' | b'?' | b'#' | b'@' | b':'))
    })
}
fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}
fn target<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a NativeCommandHttpTarget, BusinessError> {
    let host = unique(headers, header::HOST)?
        .filter(|v| !v.is_empty())
        .ok_or_else(missing)?;
    let t = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    if unique(headers, ORIGIN_HEADER)?.is_some_and(|v| v != t.origin) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        ));
    }
    Ok(t)
}
fn decode(raw: Value) -> Result<NativeCommand, BusinessError> {
    let c: NativeCommand = serde_json::from_value(raw).map_err(|_| invalid())?;
    if c.kind.is_empty()
        || c.kind.len() > 120
        || c.values.len() > 64
        || !crate::adapters::native_command_sql::supported(&c.kind)
    {
        return Err(invalid());
    }
    Ok(c)
}
async fn write(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let t = target(&state, &headers, &slug)?;
        let key = unique(&headers, KEY_HEADER)?
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                BusinessError::invalid("COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key.")
            })
            .and_then(CommandKey::new)?;
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let raw: Value = serde_json::from_slice(&body).map_err(|_| invalid())?;
        let command = decode(raw)?;
        t.backend
            .execute(NativeCommandHttpRequest {
                host: &t.host,
                holding_slug: &t.holding_slug,
                headers: &headers,
                command: &command,
                key: key.as_str(),
            })
            .await
    }
    .await;
    match result {
        Ok(v) => bounded(v),
        Err(e) => error_response(e),
    }
}
async fn company_write(
    State(state): State<Arc<AppState>>,
    Path(company_id): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let backend = state.companies.get(&company_id).ok_or_else(missing)?;
        let key = unique(&headers, KEY_HEADER)?
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                BusinessError::invalid("COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key.")
            })
            .and_then(CommandKey::new)?;
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let raw: Value = serde_json::from_slice(&body).map_err(|_| invalid())?;
        let command = decode(raw)?;
        backend
            .execute(NativeCommandHttpRequest {
                host: "",
                holding_slug: "",
                headers: &headers,
                command: &command,
                key: key.as_str(),
            })
            .await
    }
    .await;
    match result {
        Ok(v) => bounded(v),
        Err(e) => error_response(e),
    }
}

/// Parse a caller-unit read request. Reads are allow-listed against the owner
/// read registry, never the write statement registry, and carry no ledger key.
fn read_command(value: Value) -> Result<NativeCommand, BusinessError> {
    let c: NativeCommand = serde_json::from_value(value).map_err(|_| invalid())?;
    if c.kind.is_empty()
        || c.kind.len() > 120
        || c.values.is_empty()
        || c.values.len() > 64
    {
        return Err(invalid());
    }
    Ok(c)
}

/// Browser-facing caller-unit read plan. The kind is resolved against the
/// Procurement native read registry; the exact SQL is returned for the caller
/// to prepare and run on its own already-open unit. No SQL is accepted from the
/// wire, no write ledger is touched and nothing is executed here.
async fn plan_read(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let _target = target(&state, &headers, &slug)?;
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let command = read_command(value)?;
        if !crate::adapters::native_read_sql::is_read(&command.kind) {
            return Err(invalid());
        }
        let statement = crate::adapters::native_read_sql::read_statement(&command.kind)
            .ok_or_else(invalid)?;
        if command.values.len() != statement.bind_count {
            return Err(invalid());
        }
        Ok(serde_json::json!({
            "statements": [{ "sql": statement.sql, "values": command.values }]
        }))
    }
    .await;
    match result {
        Ok(data) => bounded(serde_json::json!({ "data": data })),
        Err(error) => error_response(error),
    }
}

pub fn native_company_command_router(
    companies: Vec<String>,
    backend: Arc<dyn NativeCommandHttpBackend>,
) -> Result<Router, BusinessError> {
    let pairs = companies
        .into_iter()
        .map(|c| (c, backend.clone()))
        .collect::<BTreeMap<_, _>>();
    if pairs.is_empty() {
        return Err(BusinessError::invalid(
            "PROCUREMENT_NATIVE_COMMAND_CONFIGURATION_INVALID",
            "Use at least one Company target.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/companies/{company_id}/procurement/native-command",
            post(company_write),
        )
        .with_state(Arc::new(AppState {
            targets: BTreeMap::new(),
            companies: pairs,
        })))
}

pub fn native_command_router(
    targets: Vec<NativeCommandHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut cfg = BTreeMap::new();
    for t in targets {
        if t.host.is_empty()
            || !valid_slug(&t.holding_slug)
            || !valid_origin(&t.origin)
            || cfg
                .insert((t.host.clone(), t.holding_slug.clone()), t)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PROCUREMENT_NATIVE_COMMAND_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if cfg.is_empty() {
        return Err(BusinessError::invalid(
            "PROCUREMENT_NATIVE_COMMAND_CONFIGURATION_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/procurement/native-command",
            post(write),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/procurement/command-plan",
            post(plan_read),
        )
        .with_state(Arc::new(AppState {
            targets: cfg,
            companies: BTreeMap::new(),
        })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn read_command_admits_only_bounded_read_envelopes() {
        assert!(read_command(json!({"kind":"procurement_rd_invoice_match_finance","values":["inv","co"],"now":1})).is_ok());
        assert!(read_command(json!({"kind":"","values":[1]})).is_err());
        assert!(read_command(json!({"kind":"x","values":[]})).is_err());
        let mut too_many = serde_json::Map::new();
        too_many.insert("kind".to_owned(), json!("x"));
        too_many.insert("values".to_owned(), json!((0..65).collect::<Vec<_>>()));
        assert!(read_command(Value::Object(too_many)).is_err());
    }
}
