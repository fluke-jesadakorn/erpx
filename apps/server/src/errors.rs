use std::collections::BTreeMap;
use std::time::Instant;

use erpx_domain::contracts::{ErrorCode, ErrorContext};
use serde::Serialize;
use tonic::{
    Code, Status,
    metadata::{MetadataMap, MetadataValue},
};
use uuid::Uuid;

fn error_code(code: Code) -> ErrorCode {
    match code {
        Code::Unauthenticated => ErrorCode::Unauthenticated,
        Code::PermissionDenied => ErrorCode::Forbidden,
        Code::NotFound => ErrorCode::NotFound,
        Code::AlreadyExists | Code::Aborted => ErrorCode::Conflict,
        Code::InvalidArgument => ErrorCode::InvalidArgument,
        Code::FailedPrecondition => ErrorCode::FailedPrecondition,
        Code::ResourceExhausted => ErrorCode::RateLimited,
        Code::Unavailable | Code::DeadlineExceeded => ErrorCode::Unavailable,
        _ => ErrorCode::Internal,
    }
}

fn retryable(code: Code) -> bool {
    matches!(
        code,
        Code::Unavailable | Code::DeadlineExceeded | Code::ResourceExhausted
    )
}

pub(crate) fn context(status: &Status, correlation_id: Uuid) -> ErrorContext {
    ErrorContext {
        code: error_code(status.code()),
        message: safe_message(status),
        correlation_id,
        retryable: retryable(status.code()),
        details: BTreeMap::new(),
    }
}

/// Attach the stable ERPX error metadata to a tonic status. The same fields
/// are also represented by the transport-neutral domain ErrorContext, while
/// keeping the wire addition compatible with existing tonic clients.
pub(crate) fn enrich(status: Status, correlation_id: Option<Uuid>) -> Status {
    let correlation_id = correlation_id.unwrap_or_else(Uuid::nil);
    let context = context(&status, correlation_id);
    let mut metadata: MetadataMap = status.metadata().clone();
    if let Ok(value) = MetadataValue::try_from(context.code.code_name()) {
        metadata.insert("x-erpx-error-code", value);
    }
    if let Ok(value) = MetadataValue::try_from(if context.retryable { "true" } else { "false" }) {
        metadata.insert("x-erpx-retryable", value);
    }
    if let Ok(value) = MetadataValue::try_from(context.correlation_id.to_string()) {
        metadata.insert("x-erpx-correlation-id", value);
    }
    if let Ok(value) = MetadataValue::try_from(serialized_context(&context).as_str()) {
        metadata.insert("x-erpx-error-context", value);
    }
    Status::with_metadata(status.code(), context.message, metadata)
}

fn safe_message(status: &Status) -> String {
    let fallback = match status.code() {
        Code::Unavailable | Code::DeadlineExceeded => "service unavailable",
        Code::Internal => "internal server error",
        _ => "request failed",
    };
    let message = status.message();
    if message.is_empty()
        || message.len() > 256
        || message.chars().any(char::is_control)
        || matches!(
            status.code(),
            Code::Internal | Code::Unavailable | Code::DeadlineExceeded
        )
    {
        fallback.to_owned()
    } else {
        message.to_owned()
    }
}

pub(crate) fn application(
    error: erpx_application::ApplicationError,
    correlation_id: Uuid,
) -> Status {
    let status = match error {
        erpx_application::ApplicationError::NotFound => Status::not_found("resource not found"),
        erpx_application::ApplicationError::Authorization(reason) => {
            Status::permission_denied(reason)
        }
        erpx_application::ApplicationError::Conflict(reason) => Status::aborted(reason),
        erpx_application::ApplicationError::Repository(_) => {
            Status::unavailable("persistence operation unavailable")
        }
        erpx_application::ApplicationError::Assignment(error) => {
            Status::failed_precondition(error.to_string())
        }
        erpx_application::ApplicationError::Support(error) => {
            Status::failed_precondition(error.to_string())
        }
        erpx_application::ApplicationError::RoleReview(error) => {
            Status::failed_precondition(error.to_string())
        }
    };
    enrich(status, Some(correlation_id))
}

#[derive(Debug, Serialize)]
struct HttpErrorContext {
    code: &'static str,
    message: String,
    correlation_id: Uuid,
    retryable: bool,
    details: BTreeMap<String, String>,
}

fn serialized_context(context: &ErrorContext) -> String {
    serde_json::to_string(&HttpErrorContext {
        code: context.code.code_name(),
        message: context.message.clone(),
        correlation_id: context.correlation_id,
        retryable: context.retryable,
        details: context.details.clone(),
    })
    .unwrap_or_else(|_| "{\"code\":\"internal\",\"message\":\"request failed\"}".into())
}

/// Normalize every non-success HTTP response to the transport-neutral error
/// contract. The old response body is used only as a short, already-safe
/// public message; it is never copied into logs or exposed as provider/raw
/// error data.
pub(crate) fn http_body(status: &str, body: &str, correlation_id: Uuid) -> String {
    let status_code = status
        .split_ascii_whitespace()
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(500);
    if (200..300).contains(&status_code) {
        return body.to_owned();
    }
    let code = http_error_code(status_code);
    let retryable = matches!(status_code, 408 | 425 | 429 | 500 | 502 | 503 | 504);
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .filter(|value| {
            !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
        })
        .unwrap_or_else(|| "request failed".to_owned());
    serde_json::to_string(&HttpErrorContext {
        code,
        message,
        correlation_id,
        retryable,
        details: BTreeMap::new(),
    })
    .map(|context| format!("{{\"error\":{context}}}"))
    .unwrap_or_else(|_| "{\"error\":{\"code\":\"internal\",\"message\":\"request failed\"}}".into())
}

fn http_error_code(status_code: u16) -> &'static str {
    match status_code {
        400 => "invalid_argument",
        401 => "unauthenticated",
        403 => "forbidden",
        404 => "not_found",
        409 => "conflict",
        412 => "failed_precondition",
        429 => "rate_limited",
        502..=504 => "unavailable",
        500..=599 => "internal",
        _ => "internal",
    }
}

/// Logs only method, route, correlation, and duration. No request body,
/// authorization header, token, query value, or provider payload is retained.
pub(crate) struct HttpRequestTrace {
    method: String,
    path: String,
    correlation_id: Uuid,
    started: Instant,
}

impl HttpRequestTrace {
    pub(crate) fn start(method: &str, path: &str, correlation_id: Uuid) -> Self {
        Self {
            method: method.to_owned(),
            path: path.split('?').next().unwrap_or(path).to_owned(),
            correlation_id,
            started: Instant::now(),
        }
    }
}

impl Drop for HttpRequestTrace {
    fn drop(&mut self) {
        tracing::info!(
            target: "erpx.http",
            method = %self.method,
            path = %self.path,
            correlation_id = %self.correlation_id,
            latency_ms = self.started.elapsed().as_millis() as u64,
            "http_request_completed"
        );
    }
}

trait ErrorCodeName {
    fn code_name(&self) -> &'static str;
}

impl ErrorCodeName for ErrorCode {
    fn code_name(&self) -> &'static str {
        match self {
            ErrorCode::Unauthenticated => "unauthenticated",
            ErrorCode::Forbidden => "forbidden",
            ErrorCode::NotFound => "not_found",
            ErrorCode::Conflict => "conflict",
            ErrorCode::InvalidArgument => "invalid_argument",
            ErrorCode::FailedPrecondition => "failed_precondition",
            ErrorCode::RateLimited => "rate_limited",
            ErrorCode::Unavailable => "unavailable",
            ErrorCode::Internal => "internal",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enriched_status_contains_stable_code_retry_and_correlation() {
        let correlation_id = Uuid::from_u128(42);
        let status = enrich(
            Status::unavailable("database unavailable"),
            Some(correlation_id),
        );
        assert_eq!(status.code(), Code::Unavailable);
        assert_eq!(
            status.metadata().get("x-erpx-error-code").unwrap(),
            "unavailable"
        );
        assert_eq!(status.metadata().get("x-erpx-retryable").unwrap(), "true");
        assert_eq!(
            status.metadata().get("x-erpx-correlation-id").unwrap(),
            &correlation_id.to_string()
        );
        let wire_context = status
            .metadata()
            .get("x-erpx-error-context")
            .expect("serialized context");
        let wire_context: serde_json::Value =
            serde_json::from_str(wire_context.to_str().expect("ASCII metadata"))
                .expect("JSON context");
        assert_eq!(wire_context["code"], "unavailable");
        assert_eq!(wire_context["correlation_id"], correlation_id.to_string());
        assert_eq!(
            context(&status, correlation_id).code,
            ErrorCode::Unavailable
        );
    }

    #[test]
    fn http_failures_use_the_serialized_error_context_contract() {
        let correlation_id = Uuid::from_u128(7);
        let body = http_body(
            "503 Service Unavailable",
            r#"{"error":"identity_unavailable"}"#,
            correlation_id,
        );
        let value: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(value["error"]["code"], "unavailable");
        assert_eq!(value["error"]["message"], "identity_unavailable");
        assert_eq!(value["error"]["correlation_id"], correlation_id.to_string());
        assert_eq!(value["error"]["retryable"], true);
        assert!(value["error"]["details"].is_object());
    }

    #[test]
    fn http_success_bodies_are_not_rewritten() {
        assert_eq!(
            http_body("200 OK", "{\"ok\":true}", Uuid::nil()),
            "{\"ok\":true}"
        );
    }

    #[test]
    fn internal_messages_are_redacted_before_they_cross_the_grpc_boundary() {
        let status = enrich(
            Status::internal("database password and connection string"),
            Some(Uuid::from_u128(8)),
        );
        assert_eq!(status.message(), "internal server error");
    }

    #[test]
    fn http_server_error_classification_distinguishes_internal_and_unavailable() {
        let correlation_id = Uuid::from_u128(9);
        let internal: serde_json::Value = serde_json::from_str(&http_body(
            "500 Internal Server Error",
            "{}",
            correlation_id,
        ))
        .unwrap();
        assert_eq!(internal["error"]["code"], "internal");
        let unavailable: serde_json::Value =
            serde_json::from_str(&http_body("503 Service Unavailable", "{}", correlation_id))
                .unwrap();
        assert_eq!(unavailable["error"]["code"], "unavailable");
    }
}
