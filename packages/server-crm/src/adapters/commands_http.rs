//! Shared transport facts for owner-local CRM command HTTP ingress.
//!
//! These helpers carry no owner business rule, actor, Account session, scope,
//! purchase or entitlement fact. They enforce the registered Host/Holding
//! target, the exact browser Origin, one `Idempotency-Key`, a JSON content type
//! and a bounded body, then map a [`BusinessError`] to the retained error
//! envelope. Composition still resolves the current session, selected scope,
//! authority and owner transaction behind each router's backend trait.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Request, Response, StatusCode,
    body::to_bytes, header, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

/// Bounded JSON command body. The retained TypeScript routes accept a much
/// smaller identity, activity and lead command, so 16 KiB is intentionally
/// tight rather than permissive.
pub const COMMAND_BODY_LIMIT: usize = 16 * 1024;

pub type CommandHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CrmCommandHttpReply, BusinessError>> + Send + 'a>>;

/// Owner read/command data plus the validated current context. Composition
/// supplies `context`; the router never mints it.
#[derive(Clone, Debug)]
pub struct CrmCommandHttpReply {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl CrmCommandHttpReply {
    pub fn into_value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

/// A header was repeated or was not valid ASCII. The caller maps this to its
/// slice-specific request-invalid error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeaderRejected;

pub fn unique_header(
    headers: &HeaderMap,
    name: HeaderName,
) -> Result<Option<&str>, HeaderRejected> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(HeaderRejected);
    }
    first
        .map(|value| value.to_str().map_err(|_| HeaderRejected))
        .transpose()
}

pub fn valid_origin(origin: &str) -> bool {
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
                (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit())
            })
    })
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

pub fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

pub fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

pub fn bounded(body: serde_json::Value) -> Response {
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

/// Resolve the exact registered Host/Holding target. An unknown or ambiguous
/// target is a generic 404, never a fallback to another Holding.
pub fn target<'a, T>(
    targets: &'a BTreeMap<(String, String), T>,
    headers: &HeaderMap,
    slug: &str,
    malformed: fn() -> BusinessError,
) -> Result<&'a T, BusinessError> {
    let host = unique_header(headers, header::HOST)
        .map_err(|_| malformed())?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

pub struct ParsedCommand {
    pub headers: HeaderMap,
    pub key: CommandKey,
    pub body: serde_json::Value,
}

/// Enforce the exact Origin (when present), one idempotency key, a JSON
/// content type and the bounded body. `malformed` is the router's exact
/// request-invalid error so each slice keeps a named code.
pub async fn parse_command(
    request: Request,
    origin: &str,
    malformed: fn() -> BusinessError,
) -> Result<ParsedCommand, BusinessError> {
    if unique_header(request.headers(), header::ORIGIN)
        .map_err(|_| malformed())?
        .is_some_and(|value| value != origin)
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        ));
    }
    let key = unique_header(
        request.headers(),
        HeaderName::from_static("idempotency-key"),
    )
    .map_err(|_| malformed())?
    .filter(|value| !value.is_empty())
    .ok_or_else(|| BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required."))
    .and_then(CommandKey::new)?;
    if !unique_header(request.headers(), header::CONTENT_TYPE)
        .map_err(|_| malformed())?
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
        })
    {
        return Err(malformed());
    }
    let headers = request.headers().clone();
    let body = to_bytes(request.into_body(), COMMAND_BODY_LIMIT)
        .await
        .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller CRM command."))?;
    let body = serde_json::from_slice::<serde_json::Value>(&body).map_err(|_| malformed())?;
    Ok(ParsedCommand { headers, key, body })
}

/// Validate the registered target list once at router construction.
pub fn configure<T, F>(
    targets: Vec<T>,
    code: &'static str,
    identity: F,
) -> Result<BTreeMap<(String, String), T>, BusinessError>
where
    F: Fn(&T) -> (&str, &str, &str),
{
    let mut configured = BTreeMap::new();
    for target in targets {
        let (host, holding_slug, origin) = identity(&target);
        if host.is_empty()
            || !valid_slug(holding_slug)
            || !valid_origin(origin)
            || configured
                .insert((host.to_owned(), holding_slug.to_owned()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                code,
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(configured)
}
