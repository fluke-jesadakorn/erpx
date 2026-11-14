//! Shared transport facts for the owner-local Support HTTP routers.
//!
//! These helpers never resolve an Account session, Company scope, purchase
//! entitlement or owner transaction. They validate the registered target,
//! origin/idempotency/bounded JSON and map `BusinessError` to the wire shape.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Request, Response, StatusCode,
    body::to_bytes, header, json, serde_json::Value,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin};

pub type SupportHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginPolicy {
    /// The request must carry the exact registered browser origin.
    Required,
    /// The origin header is optional; a supplied value must still match.
    Optional,
}

/// A registered Host/Holding target. The backend owns all session, scope and
/// transaction work behind the target.
pub trait SupportHttpTargetSpec {
    fn host(&self) -> &str;
    fn holding_slug(&self) -> &str;
    fn origin(&self) -> &str;
}

pub fn configuration_invalid(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "Use exact distinct registered Holding targets.")
}

pub fn configure_targets<T: SupportHttpTargetSpec>(
    targets: Vec<T>,
    code: &'static str,
) -> Result<BTreeMap<(String, String), T>, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host().is_empty()
            || target.holding_slug().is_empty()
            || !valid_origin(target.origin())
            || configured
                .insert(
                    (target.host().to_owned(), target.holding_slug().to_owned()),
                    target,
                )
                .is_some()
        {
            return Err(configuration_invalid(code));
        }
    }
    Ok(configured)
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn validation_failed() -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", "Provide a valid Support request.")
}

pub fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(BusinessError::invalid(
            "SUPPORT_REQUEST_INVALID",
            "Use one unambiguous request header value.",
        ));
    }
    first
        .map(|value| {
            value.to_str().map_err(|_| {
                BusinessError::invalid("SUPPORT_REQUEST_INVALID", "Use a valid request header.")
            })
        })
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

fn target_host<'a>(headers: &'a HeaderMap) -> Result<&'a str, BusinessError> {
    unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)
}

pub fn selected_target<'a, T>(
    targets: &'a BTreeMap<(String, String), T>,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a T, BusinessError> {
    let host = target_host(headers)?;
    targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

pub struct ParsedSupportBody {
    pub headers: HeaderMap,
    pub body: Value,
}

/// Validate origin and bounded JSON for a preview/read-only POST with no command
/// key. A supplied origin must still match the registered target.
pub async fn parse_json_body(
    request: Request,
    target_origin: &str,
    origin_policy: OriginPolicy,
    body_limit: usize,
) -> Result<ParsedSupportBody, BusinessError> {
    let origin = unique_header(request.headers(), header::ORIGIN)?;
    match origin_policy {
        OriginPolicy::Required if origin != Some(target_origin) => {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        OriginPolicy::Optional if origin.is_some_and(|origin| origin != target_origin) => {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        _ => {}
    }
    if let Some(length) = unique_header(request.headers(), header::CONTENT_LENGTH)?
        && length
            .parse::<usize>()
            .is_ok_and(|length| length > body_limit)
    {
        return Err(too_large(body_limit));
    }
    let headers = request.headers().clone();
    let bytes = to_bytes(request.into_body(), body_limit)
        .await
        .map_err(|_| too_large(body_limit))?;
    let body = serde_json::from_slice::<Value>(&bytes)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Use valid JSON."))?;
    Ok(ParsedSupportBody { headers, body })
}

pub struct ParsedSupportCommand {
    pub headers: HeaderMap,
    pub key: CommandKey,
    pub body: Value,
}

/// Validate origin, idempotency key and bounded JSON before any business work.
pub async fn parse_command(
    request: Request,
    target_origin: &str,
    origin_policy: OriginPolicy,
    body_limit: usize,
) -> Result<ParsedSupportCommand, BusinessError> {
    let origin = unique_header(request.headers(), header::ORIGIN)?;
    match origin_policy {
        OriginPolicy::Required if origin != Some(target_origin) => {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        OriginPolicy::Optional if origin.is_some_and(|origin| origin != target_origin) => {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        _ => {}
    }
    let key = unique_header(
        request.headers(),
        HeaderName::from_static("idempotency-key"),
    )?
    .filter(|value| !value.is_empty())
    .ok_or_else(|| {
        BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "Use a valid Idempotency-Key for this action.",
        )
    })
    .and_then(CommandKey::new)?;
    if let Some(length) = unique_header(request.headers(), header::CONTENT_LENGTH)?
        && length
            .parse::<usize>()
            .is_ok_and(|length| length > body_limit)
    {
        return Err(too_large(body_limit));
    }
    let headers = request.headers().clone();
    let bytes = to_bytes(request.into_body(), body_limit)
        .await
        .map_err(|_| too_large(body_limit))?;
    let body = serde_json::from_slice::<Value>(&bytes)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Use valid JSON."))?;
    Ok(ParsedSupportCommand { headers, key, body })
}

fn too_large(limit: usize) -> BusinessError {
    // `BusinessError` has no 413 kind; `error_response` maps this exact code.
    BusinessError::invalid(
        "BODY_TOO_LARGE",
        if limit >= 100_000 {
            "Shorten the content before saving."
        } else {
            "Use a smaller Support command."
        },
    )
}

/// Parse either no query or exactly one bounded `ticketId` selector.
pub fn ticket_selection_query(raw: Option<&str>) -> Result<Option<String>, BusinessError> {
    let Some(raw) = raw.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let mut selected = None;
    for pair in raw.split('&') {
        let (key, value) = pair.split_once('=').ok_or_else(missing)?;
        if key != "ticketId" || selected.is_some() {
            return Err(missing());
        }
        if value.is_empty()
            || value.len() > 200
            || value
                .bytes()
                .any(|byte| byte.is_ascii_control() || matches!(byte, b'%' | b'+' | b' '))
        {
            return Err(validation_failed());
        }
        selected = Some(value.to_owned());
    }
    Ok(selected)
}

/// Reject all query selectors on endpoints that accept none.
pub fn no_query(raw: Option<&str>) -> Result<(), BusinessError> {
    match raw {
        None | Some("") => Ok(()),
        Some(_) => Err(validation_failed()),
    }
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
    let status = if error.code == "BODY_TOO_LARGE" {
        StatusCode::PAYLOAD_TOO_LARGE
    } else {
        status(error.kind)
    };
    (
        status,
        Json(json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

pub fn data_response(value: Value, status: StatusCode) -> Response {
    let mut response = (status, Json(json!({"data": value}))).into_response();
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

pub fn ok(value: Value) -> Response {
    data_response(value, StatusCode::OK)
}

pub fn created(value: Value) -> Response {
    data_response(value, StatusCode::CREATED)
}

pub fn not_found_source() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "RECORD_NOT_FOUND",
        "The notification source is not available.",
    )
}

#[cfg(test)]
pub(crate) fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }

    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_and_selection_validation_is_strict() {
        assert!(valid_origin("https://acme.example.test"));
        assert!(valid_origin("https://acme.example.test:8443"));
        for invalid in [
            "http://acme.example.test",
            "https://",
            "https://acme.example.test/path",
            "https://acme.example.test:",
            "https://acme.example.test:abc",
        ] {
            assert!(!valid_origin(invalid), "{invalid}");
        }
        assert_eq!(
            ticket_selection_query(Some("ticketId=ticket-1")).unwrap(),
            Some("ticket-1".into())
        );
        assert_eq!(ticket_selection_query(None).unwrap(), None);
        for invalid in [
            "ticketId=",
            "ticketId=a&ticketId=b",
            "bindingId=binding-1",
            "ticketId=a&other=b",
            "ticketId=a%20b",
        ] {
            assert!(ticket_selection_query(Some(invalid)).is_err(), "{invalid}");
        }
        assert!(no_query(None).is_ok());
        assert!(no_query(Some("ticketId=a")).is_err());
    }

    #[test]
    fn errors_map_to_the_expected_status_and_shape() {
        let response = error_response(BusinessError::new(
            FailureKind::NotFound,
            "NOT_FOUND",
            "Not found.",
        ));
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let response = error_response(BusinessError::invalid(
            "BODY_TOO_LARGE",
            "Use a smaller Support command.",
        ));
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let response = ok(json!({"ready": true}));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
    }
}
