//! Shared transport facts for Collaboration owner HTTP ingress.
//!
//! These helpers only validate the exact registered Holding host/origin, one
//! bounded command key, a bounded JSON object and stable error envelopes. They
//! never resolve a session, Company membership or owner authority.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Request, Response, StatusCode,
    body::to_bytes, header,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::Serialize;
use serde_json::{Value, json};

pub(crate) const BODY_LIMIT: usize = 192_000;

pub(crate) fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "COLLABORATION_REQUEST_INVALID",
        "Use one unambiguous request header value.",
    )
}

pub(crate) fn content_invalid() -> BusinessError {
    BusinessError::invalid(
        "COLLABORATION_INPUT_INVALID",
        "Provide a valid collaboration value.",
    )
}

pub(crate) fn unique_header(
    headers: &HeaderMap,
    name: HeaderName,
) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

pub(crate) fn valid_origin(host: &str, origin: &str) -> bool {
    let expected = format!("https://{host}");
    if origin != expected || host.is_empty() {
        return false;
    }
    if !host
        .bytes()
        .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    {
        return true;
    }
    // The keyed contributor workspace uses its exact local HTTPS port.
    host.strip_suffix(":3443")
        .is_some_and(|name| name.ends_with(".erpx.localhost"))
}

fn key_valid(value: &str) -> bool {
    let len = value.len();
    (8..=180).contains(&len)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-'))
}

pub(crate) fn command_key(headers: &HeaderMap) -> Result<CommandKey, BusinessError> {
    let value = unique_header(headers, HeaderName::from_static("idempotency-key"))?
        .filter(|value| key_valid(value))
        .ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "A valid Idempotency-Key is required.",
            )
        })?;
    CommandKey::new(value)
}

pub(crate) fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

pub(crate) fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

pub(crate) fn bounded<T: Serialize>(body: T) -> Response {
    let mut response = (StatusCode::OK, Json(json!({ "data": body }))).into_response();
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

pub(crate) async fn json_body(request: Request) -> Result<Value, BusinessError> {
    let body = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| {
            BusinessError::invalid("INPUT_TOO_LARGE", "Shorten the content before saving.")
        })?;
    let raw: Value = serde_json::from_slice(&body).map_err(|_| {
        BusinessError::invalid("INVALID_JSON", "Provide a valid collaboration command.")
    })?;
    if !raw.is_object() {
        return Err(content_invalid());
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::body::Body;

    #[test]
    fn origins_require_the_exact_https_holding_or_local_port() {
        assert!(valid_origin(
            "acme.example.test",
            "https://acme.example.test"
        ));
        assert!(valid_origin(
            "acme.erpx.localhost:3443",
            "https://acme.erpx.localhost:3443"
        ));
        assert!(!valid_origin(
            "acme.example.test",
            "http://acme.example.test"
        ));
        assert!(!valid_origin("acme.example.test", "https://evil.test"));
        assert!(!valid_origin("acme:3443", "https://acme:3443"));
    }

    #[test]
    fn command_keys_are_bounded_and_unambiguous() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "idempotency-key",
            HeaderValue::from_static("collab-key-0001"),
        );
        assert_eq!(command_key(&headers).unwrap().as_str(), "collab-key-0001");
        headers.append(
            "idempotency-key",
            HeaderValue::from_static("collab-key-0002"),
        );
        assert_eq!(
            command_key(&headers).unwrap_err().code,
            "COLLABORATION_REQUEST_INVALID"
        );
        let mut headers = HeaderMap::new();
        headers.insert("idempotency-key", HeaderValue::from_static("short"));
        assert_eq!(
            command_key(&headers).unwrap_err().code,
            "COMMAND_KEY_REQUIRED"
        );
    }

    #[tokio::test]
    async fn bodies_are_bounded_json_objects() {
        let request = Request::builder().body(Body::from(r#"{"a":1}"#)).unwrap();
        assert_eq!(json_body(request).await.unwrap()["a"], 1);
        let request = Request::builder().body(Body::from("[]")).unwrap();
        assert_eq!(
            json_body(request).await.unwrap_err().code,
            "COLLABORATION_INPUT_INVALID"
        );
        let request = Request::builder().body(Body::from("not json")).unwrap();
        assert_eq!(json_body(request).await.unwrap_err().code, "INVALID_JSON");
    }
}
